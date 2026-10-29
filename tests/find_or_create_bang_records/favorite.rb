class Favorite < ApplicationRecord
  belongs_to :article
  belongs_to :user

  def self.add(article, user)
    Favorite.find_or_create_by!(article: article, user: user)
  end
end
